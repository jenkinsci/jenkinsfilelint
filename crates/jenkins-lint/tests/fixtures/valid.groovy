@Library('shared@main') _

def label = 'linux'

pipeline {
    agent { docker { image 'maven:3.9'; args '-v $HOME/.m2:/root/.m2' } }
    options {
        timeout(time: 1, unit: 'HOURS')
        buildDiscarder(logRotator(numToKeepStr: '10'))
    }
    parameters {
        string(name: 'TARGET', defaultValue: 'all', description: 'make target')
        booleanParam(name: 'DEPLOY', defaultValue: false)
    }
    environment {
        APP = 'demo'
        VERSION = "1.0.${BUILD_NUMBER}"
        CREDS = credentials('deploy-creds')
    }
    stages {
        stage('Build') {
            when { anyOf { branch 'main'; changeRequest() } }
            steps {
                sh script: 'mvn -B package', returnStatus: false
                junit allowEmptyResults: true, testResults: '**/target/*.xml'
                dir('sub') {
                    sh "make ${params.TARGET}"
                }
                script {
                    def out = sh(script: 'git rev-parse HEAD', returnStdout: true).trim()
                    if (out.startsWith('a')) {
                        currentBuild.description = out
                    }
                    [1, 2, 3].each { n -> echo "n=${n}" }
                }
            }
        }
        stage('Tests') {
            parallel {
                stage('Unit') { steps { sh 'make unit' } }
                stage('Lint') {
                    agent { label label }
                    steps { sh 'make lint' }
                }
            }
        }
        stage('Matrix') {
            matrix {
                axes {
                    axis { name 'OS'; values 'linux', 'windows' }
                }
                stages {
                    stage('Run') { steps { echo "on ${OS}" } }
                }
            }
        }
        stage('Deploy') {
            when { expression { params.DEPLOY } }
            input { message 'Deploy?' }
            steps {
                withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'U', passwordVariable: 'P')]) {
                    sh './deploy.sh'
                }
            }
        }
    }
    post {
        always { archiveArtifacts artifacts: 'target/*.jar', allowEmptyArchive: true }
        failure { echo 'failed' }
    }
}

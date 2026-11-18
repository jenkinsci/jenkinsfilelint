#!/usr/bin/env groovy
@Library('my-shared-lib@main') _

import groovy.json.JsonSlurper

def helper(String name) {
    return "Hello, ${name}"
}

def VERSION = '1.0.0'

pipeline {
    agent { label 'linux && docker' }
    options {
        timeout(time: 1, unit: 'HOURS')
        retry(2)
        buildDiscarder(logRotator(numToKeepStr: '10', artifactNumToKeepStr: '5'))
        disableConcurrentBuilds()
        skipDefaultCheckout()
    }
    parameters {
        string(name: 'BRANCH', defaultValue: 'main', description: 'Branch to build')
        booleanParam(name: 'DEPLOY', defaultValue: false, description: 'Deploy?')
        choice(name: 'ENV', choices: ['dev', 'staging', 'prod'], description: 'Target')
        text(name: 'NOTES', defaultValue: '', description: 'Notes')
        password(name: 'SECRET', defaultValue: '', description: 'Secret')
    }
    triggers {
        cron('H 2 * * 1-5')
        pollSCM('H/15 * * * *')
    }
    environment {
        APP = 'demo'
        FULL = "${APP}-${env.BUILD_NUMBER}"
        CREDS = credentials('my-creds')
    }
    stages {
        stage('Checkout') {
            steps {
                checkout scm
            }
        }
        stage('Build') {
            when {
                anyOf {
                    branch 'main'
                    branch pattern: 'release/.*', comparator: 'REGEXP'
                }
            }
            steps {
                sh 'make build'
                sh script: 'git rev-parse HEAD', returnStdout: true
                script {
                    def out = sh(script: 'echo hi', returnStdout: true).trim()
                    if (out == 'hi') {
                        echo helper('world')
                    } else {
                        error "unexpected: ${out}"
                    }
                }
            }
        }
        stage('Test') {
            parallel {
                stage('Unit') {
                    steps { sh 'make test' }
                }
                stage('Lint') {
                    agent { docker { image 'python:3.12' } }
                    steps { sh 'make lint' }
                }
            }
        }
        stage('Deploy') {
            when {
                beforeAgent true
                allOf {
                    expression { params.DEPLOY }
                    environment name: 'ENV', value: 'prod'
                }
            }
            input {
                message 'Deploy to prod?'
                ok 'Yes'
                parameters {
                    string(name: 'REASON', defaultValue: '', description: 'Why')
                }
            }
            steps {
                echo "Deploying ${VERSION} because ${REASON}"
            }
        }
    }
    post {
        always { echo 'always' }
        success { echo 'ok' }
        failure { echo 'failed' }
        unstable { echo 'unstable' }
        changed { echo 'changed' }
        fixed { echo 'fixed' }
        regression { echo 'regression' }
        aborted { echo 'aborted' }
        unsuccessful { echo 'unsuccessful' }
        cleanup { deleteDir() }
    }
}

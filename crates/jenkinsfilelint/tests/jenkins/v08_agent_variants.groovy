pipeline {
    agent none
    stages {
        stage('Label') {
            agent { label 'linux' }
            steps { echo 'label' }
        }
        stage('Node') {
            agent {
                node {
                    label 'linux'
                    customWorkspace '/tmp/ws'
                }
            }
            steps { echo 'node' }
        }
        stage('Docker') {
            agent {
                docker {
                    image 'maven:3-eclipse-temurin-17'
                    args '-v $HOME/.m2:/root/.m2'
                    reuseNode true
                }
            }
            steps { sh 'mvn -v' }
        }
        stage('Dockerfile') {
            agent {
                dockerfile {
                    filename 'Dockerfile.build'
                    dir 'build'
                    additionalBuildArgs '--build-arg X=1'
                }
            }
            steps { sh 'make' }
        }
    }
}

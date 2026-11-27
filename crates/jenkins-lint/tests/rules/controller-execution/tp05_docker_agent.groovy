pipeline {
    agent {
        docker {
            image 'maven:3.9.9-eclipse-temurin-21'
            label 'master'
        }
    }
    stages {
        stage('Build') {
            steps {
                sh 'mvn -B verify'
            }
        }
    }
}

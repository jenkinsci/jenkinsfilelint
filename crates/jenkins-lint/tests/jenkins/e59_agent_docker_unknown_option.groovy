pipeline {
    agent {
        docker {
            image 'maven:3'
            pull true
        }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}

pipeline {
    agent any
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}

def other() {
    pipeline {
        agent any
        stages {
            stage('B') {
                steps { echo 'b' }
            }
            stage('B') {
                steps { echo 'c' }
            }
        }
    }
}

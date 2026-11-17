pipeline {
    agent any
    stages {
        stage('M') {
            matrix {
                axes {
                    axis {
                        name 'OS'
                        notValues 'linux'
                    }
                }
                stages {
                    stage('S') {
                        steps { echo 'x' }
                    }
                }
            }
        }
    }
}

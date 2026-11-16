pipeline {
    agent any
    stages {
        stage('M') {
            matrix {
                failFast true
                axes {
                    axis {
                        name 'OS'
                        values 'linux', 'mac'
                    }
                }
                stages {
                    stage('T') { steps { echo 'x' } }
                }
            }
        }
    }
}
